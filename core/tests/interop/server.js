const express = require('express')
const { braidify } = require('../../reference/braid-http/braid-http-server.js')

const app = express()
const port = 3009

app.use(braidify)

let state = {
    version: 'v0',
    data: 'initial state'
}

app.get('/test', (req, res) => {
    if (req.subscribe) {
        res.startBraid({
            version: state.version,
            buffer: true
        })
        res.sendUpdate({
            version: state.version,
            data: JSON.stringify(state.data)
        })
        
        // Send a second update after 1 second
        setTimeout(() => {
            state.version = 'v1'
            state.data = 'updated state'
            res.sendUpdate({
                version: state.version,
                data: JSON.stringify(state.data)
            })
        }, 1000)

        // Close after 3 seconds for test completion
        setTimeout(() => {
            res.end()
        }, 3000)
    } else {
        res.sendBraid({
            version: state.version,
            data: JSON.stringify(state.data)
        })
    }
})

app.listen(port, () => {
    console.log(`Interop JS server listening on port ${port}`)
})
